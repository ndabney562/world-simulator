using System;
using System.Diagnostics;
using System.Threading;
using UnityEngine;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>Coalesces host timestamps, without accumulating or scaling simulation steps.</summary>
    public sealed class SliceHostClock
    {
        readonly ulong frequency,interval;
        long previous;
        ulong remainder;
        public bool Paused { get; set; }
        public SliceHostClock(long frequency,long timestamp)
        {
            if(frequency<=0) throw new ArgumentOutOfRangeException(nameof(frequency));
            this.frequency=(ulong)frequency;interval=this.frequency/20+(this.frequency%20==0?0UL:1UL);previous=timestamp;
        }
        /// <summary>At most20 periodic submissions/second. Force closes a control boundary at the old scale.</summary>
        public bool Advance(long timestamp,bool force,out ulong microseconds)
        {
            microseconds=0;
            long elapsed=checked(timestamp-previous);
            if(elapsed<0) throw new InvalidOperationException("Host monotonic clock moved backwards.");
            if(Paused) {previous=timestamp;return false;}
            ulong delta=(ulong)elapsed;
            if(!force&&delta<interval) return false;
            // Convert stopwatch units only. Rust owns the simulation cap/drop/remainder.
            ulong fraction=checked((delta%frequency)*1000000+remainder);
            microseconds=checked(delta/frequency*1000000+fraction/frequency);
            remainder=fraction%frequency;previous=timestamp;
            return microseconds!=0;
        }
    }
    /// <summary>Worker supplies elapsed integer host microseconds; Rust owns pacing, scale and LOD.</summary>
    public sealed class SliceHost : IDisposable
    {
        readonly NativeSlice world;
        readonly Thread worker;
        readonly object pacingGate=new object();
        readonly SliceHostClock clock;
        volatile bool stop;
        public volatile Exception Failure;
        public SliceGeometry Geometry => world.Geometry;
        public int Population => world.Population;
        public SliceHost(ulong seed,long start) : this(new NativeSlice(seed,start))
        {
        }
        public SliceHost(NativeSlice loadedSlice)
        {
            world=loadedSlice ?? throw new ArgumentNullException(nameof(loadedSlice));
            clock=new SliceHostClock(Stopwatch.Frequency,Stopwatch.GetTimestamp());
#if !UNITY_WEBGL || UNITY_EDITOR
            worker=new Thread(Run) {Name="WorldSim real slice",IsBackground=true};
            try {worker.Start();} catch {world.Dispose();throw;}
#endif
        }
        public static SliceHost Load(string path) => new SliceHost(NativeSlice.Load(path));
        void Run()
        {
            try
            {
                while(!stop)
                {
                    lock(pacingGate) SubmitElapsed(Stopwatch.GetTimestamp(),false);
                    Thread.Sleep(10);
                }
            }
            catch(ObjectDisposedException) { }
            catch(Exception error) {Failure=error;}
        }
        void SubmitElapsed(long timestamp,bool force)
        {
            if(clock.Advance(timestamp,force,out ulong elapsed)) world.HostElapsed(elapsed);
        }
        public void SetScale(uint scale)
        {
            if(scale!=0&&scale!=1&&scale!=10&&scale!=100) throw new ArgumentOutOfRangeException(nameof(scale));
            lock(pacingGate)
            {
                // Attribute the old interval to the old scale, including immediately
                // before pausing. No paused wall time can leak into the resumed scale.
                SubmitElapsed(Stopwatch.GetTimestamp(),true);
                world.SetScale(scale);clock.Paused=scale==0;
            }
        }
        public SliceInspection Inspect(ulong id)=>world.Inspect(id);
        public SliceIllnessSample Illness(ulong id,ulong root=0)
        {
            lock(pacingGate) {SubmitElapsed(Stopwatch.GetTimestamp(),true);return world.Illness(id,root);}
        }
        public void SetCamera(Vector3 target,double radius) {world.SetCamera(target,radius);}
        /// <summary>Includes lock wait, fill, coherent sky and decode; costs share the captured tick window.</summary>
        public double Fill(SliceFrame frame,out SliceAdvanceStats stats)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            lock(pacingGate) SubmitElapsed(Stopwatch.GetTimestamp(),false);
#endif
            long begin=Stopwatch.GetTimestamp();world.Fill(frame,out stats);
            return (Stopwatch.GetTimestamp()-begin)*1000.0/Stopwatch.Frequency;
        }
        public void Save(string path)
        {
            lock(pacingGate)
            {
                SubmitElapsed(Stopwatch.GetTimestamp(),true);
                world.Save(path);
            }
        }
        public void SaveBackground(string path)
        {
            lock(pacingGate)
            {
                SubmitElapsed(Stopwatch.GetTimestamp(),true);
#if UNITY_WEBGL && !UNITY_EDITOR
                world.Save(path);
#else
                world.SaveBackground(path);
#endif
            }
        }
        public uint SaveStatus() => world.SaveStatus();
        public void Dispose()
        {
            stop=true;worker?.Join();world.Dispose();
        }
    }
}
