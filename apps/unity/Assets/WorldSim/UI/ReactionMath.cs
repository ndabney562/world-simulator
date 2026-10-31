using UnityEngine;

namespace WorldSim.UI
{
    /// <summary>A one-shot effect shown where a power lands.</summary>
    public enum EffectKind {Bless,Curse,Gathering}

    /// <summary>Pure timing and shape rules for cast effects and status icons, in real seconds.</summary>
    public static class ReactionMath
    {
        public const float GatheringRadius=8,RainHeight=14,RainSpeed=12;
        public const double EaseSeconds=0.6;
        public static double Duration(EffectKind kind)=>kind==EffectKind.Bless?1.5:kind==EffectKind.Curse?1.2:2.5;
        public static bool Alive(EffectKind kind,double age)=>age>=0&&age<Duration(kind);
        /// <summary>Grows to full size over the first quarter, then shrinks to nothing.</summary>
        public static float Size(EffectKind kind,double age)
        {
            if(!Alive(kind,age)) return 0;
            double t=age/Duration(kind);
            return (float)(t<0.25?t/0.25:1-(t-0.25)/0.75);
        }
        /// <summary>The gathering ring widens steadily from the home to its full radius.</summary>
        public static float RingRadius(double age)=>Alive(EffectKind.Gathering,age)?(float)(age/Duration(EffectKind.Gathering))*GatheringRadius:0;
        /// <summary>Smoothstep camera ease fraction, 0 at the start and 1 from <see cref="EaseSeconds"/> on.</summary>
        public static float Ease(double elapsed)
        {
            float t=Mathf.Clamp01((float)(elapsed/EaseSeconds));
            return t*t*(3-2*t);
        }
        /// <summary>Height of one rain streak above the ground; <paramref name="phase"/> in [0,1) staggers streaks.</summary>
        public static float RainY(double seconds,float phase)
        {
            double y=RainHeight-(seconds*RainSpeed+phase*RainHeight)%RainHeight;
            return (float)y;
        }
        /// <summary>Icons bob 0.15 m at 1.5 Hz so they read as markers, not scenery.</summary>
        public static float Bob(double seconds)=>0.15f*Mathf.Sin((float)(seconds*1.5*2*Mathf.PI));
    }
}
