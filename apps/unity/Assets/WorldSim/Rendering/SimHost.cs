using System;
using System.Diagnostics;
using System.Threading;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>
    /// Owns one native world and the worker thread that paces it. The worker flushes commands and advances
    /// simulation steps; the main thread submits commands and decodes one snapshot per frame. Both go through
    /// the single native handle's serializing lock, so a long step can delay the main thread's fill. That
    /// wait is part of the fill time <see cref="Fill"/> reports, which the TP-011 note uses for the double-buffering question.
    /// </summary>
    public sealed class SimHost : IDisposable
    {
        readonly NativeWorld world;
        readonly Thread worker;
        volatile bool stop;
        long advanceTicks, advanceMaxTicks, advanceCount, tickTotal;
        public int ResidentCount => world.ResidentCount;
        public int InstanceCount => world.InstanceCount;
        public uint CommittedScale => world.CommittedScale;
        /// <summary>Total simulation steps taken by the worker.</summary>
        public long Steps => Interlocked.Read(ref tickTotal);
        public SimHost(ulong seed, int residentsPerBuilding)
        {
            world = new NativeWorld(seed, residentsPerBuilding);
            worker = new Thread(Run) { Name = "WorldSim core", IsBackground = true };
            worker.Start();
        }
        void Run()
        {
            double remainder = 0;
            var clock = Stopwatch.StartNew();
            double previous = 0;
            try
            {
                while (!stop)
                {
                    double now = clock.Elapsed.TotalSeconds;
                    double elapsed = now - previous; previous = now;
                    uint scale = world.FlushCommands();
                    uint steps = BridgeDemo.ConsumeHostElapsed(ref remainder, elapsed, scale);
                    if (steps > 0)
                    {
                        long begin = Stopwatch.GetTimestamp();
                        world.Advance(steps);
                        long took = Stopwatch.GetTimestamp() - begin;
                        Interlocked.Add(ref advanceTicks, took);
                        Interlocked.Add(ref advanceCount, steps);
                        Interlocked.Add(ref tickTotal, steps);
                        long perStep = took / steps;
                        long seen;
                        while ((seen = Interlocked.Read(ref advanceMaxTicks)) < perStep &&
                               Interlocked.CompareExchange(ref advanceMaxTicks, perStep, seen) != seen) { }
                    }
                    Thread.Sleep(2);
                }
            }
            catch (ObjectDisposedException) { }
            catch (Exception error) { Failure = error; }
        }
        /// <summary>Set if the worker stopped on an error; checked by the host MonoBehaviour every frame.</summary>
        public volatile Exception Failure;
        public void SetScale(uint scale) { world.SubmitTimeScale(scale); }
        /// <summary>Decode the newest committed state into <paramref name="frame"/>; returns lock wait plus native fill plus decode, in ms.</summary>
        public double Fill(RenderFrame frame)
        {
            long begin = Stopwatch.GetTimestamp();
            world.FillSnapshot(frame);
            return (Stopwatch.GetTimestamp() - begin) * 1000.0 / Stopwatch.Frequency;
        }
        /// <summary>Mean and worst per-step native advance time since the previous call, in ms; resets the window.</summary>
        public void TakeAdvanceStats(out double meanMs, out double maxMs)
        {
            long total = Interlocked.Exchange(ref advanceTicks, 0);
            long count = Interlocked.Exchange(ref advanceCount, 0);
            long max = Interlocked.Exchange(ref advanceMaxTicks, 0);
            double scale = 1000.0 / Stopwatch.Frequency;
            meanMs = count == 0 ? 0 : total * scale / count;
            maxMs = max * scale;
        }
        public void Dispose()
        {
            stop = true;
            worker.Join();
            world.Dispose();
        }
    }
}
