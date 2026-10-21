using System;
using System.Diagnostics;
using System.Threading;
using UnityEngine;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>Worker supplies elapsed integer host microseconds; Rust owns pacing, scale and LOD.</summary>
    public sealed class SliceHost : IDisposable
    {
        readonly NativeSlice world;
        readonly Thread worker;
        volatile bool stop;
        public volatile Exception Failure;
        public SliceGeometry Geometry => world.Geometry;
        public int Population => world.Population;
        public SliceHost(ulong seed,long start)
        {
            world=new NativeSlice(seed,start);
            worker=new Thread(Run) {Name="WorldSim real slice",IsBackground=true};
            try {worker.Start();} catch {world.Dispose();throw;}
        }
        void Run()
        {
            long previous=Stopwatch.GetTimestamp();ulong remainder=0;
            try
            {
                while(!stop)
                {
                    long now=Stopwatch.GetTimestamp(),elapsed=now-previous;previous=now;
                    if(elapsed<0) throw new InvalidOperationException("Host monotonic clock moved backwards.");
                    // This remainder converts stopwatch units only. It is not a second
                    // simulation-step accumulator; core owns that cap/drop/remainder.
                    ulong frequency=(ulong)Stopwatch.Frequency,delta=(ulong)elapsed;
                    ulong fraction=checked((delta%frequency)*1000000+remainder);
                    ulong microseconds=checked(delta/frequency*1000000+fraction/frequency);
                    remainder=fraction%frequency;world.HostElapsed(microseconds);Thread.Sleep(2);
                }
            }
            catch(ObjectDisposedException) { }
            catch(Exception error) {Failure=error;}
        }
        public void SetScale(uint scale) {world.SetScale(scale);}
        public void SetCamera(Vector3 target,double radius) {world.SetCamera(target,radius);}
        /// <summary>Includes lock wait, fill, coherent sky and decode; costs share the captured tick window.</summary>
        public double Fill(SliceFrame frame,out SliceAdvanceStats stats)
        {
            long begin=Stopwatch.GetTimestamp();world.Fill(frame,out stats);
            return (Stopwatch.GetTimestamp()-begin)*1000.0/Stopwatch.Frequency;
        }
        public void Dispose()
        {
            stop=true;worker.Join();world.Dispose();
        }
    }
}
