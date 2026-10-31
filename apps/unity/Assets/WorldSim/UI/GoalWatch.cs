using System.Collections.Generic;
using WorldSim.Bridge;

namespace WorldSim.UI
{
    /// <summary>Pure goal-completion detector: reports goals that turned done between two polls of the same day.
    /// The first poll, a day change and a reset (after a load) report nothing.</summary>
    public sealed class GoalWatch
    {
        /// <summary>The sim gives three goals a day.</summary>
        public const int MaxGoals=3;
        readonly bool[] done=new bool[MaxGoals];
        long day;bool have;
        public void Reset()
        {
            have=false;System.Array.Clear(done,0,done.Length);
        }
        /// <summary>Appends each goal newly done since the previous poll of the same day to <paramref name="completed"/>.</summary>
        public void Update(SliceGod g,List<SliceGoal> completed)
        {
            int n=System.Math.Min(g.Goals.Length,MaxGoals);
            bool same=have&&g.Day==day;
            for(int i=0;i<n;i++)
            {
                if(same&&g.Goals[i].Done&&!done[i]) completed.Add(g.Goals[i]);
                done[i]=g.Goals[i].Done;
            }
            for(int i=n;i<MaxGoals;i++) done[i]=false;
            day=g.Day;have=true;
        }
    }
}
