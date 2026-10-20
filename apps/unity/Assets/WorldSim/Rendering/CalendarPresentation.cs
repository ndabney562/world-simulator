using System;
using System.Globalization;
using UnityEngine;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>Presents a core calendar/sky record; owns neither a clock nor a solar calculation.</summary>
    public sealed class CalendarPresentation
    {
        readonly Light light;
        CalendarSky last;
        bool hasLabel;
        public string Text { get; private set; } = "";
        public CalendarPresentation(Light directionalLight)
        {
            if (directionalLight == null || directionalLight.type != LightType.Directional)
                throw new ArgumentException("Calendar presentation needs the scene's directional light.");
            light = directionalLight;
        }
        public void Apply(CalendarSky sky)
        {
            light.enabled = sky.Supported && sky.ElevationDegrees > 0;
            if (light.enabled) light.transform.rotation = Quaternion.LookRotation(-sky.TowardSun, Vector3.up);
            // UTC labels distinguish leap seconds and every integer-range boundary;
            // do not use a rounded float or an overflowing second-start subtraction.
            if (hasLabel && last.UtcYear == sky.UtcYear && last.UtcMonth == sky.UtcMonth && last.UtcDay == sky.UtcDay &&
                last.UtcHour == sky.UtcHour && last.UtcMinute == sky.UtcMinute && last.UtcSecond == sky.UtcSecond && last.ScaleTag == sky.ScaleTag)
                return;
            last = sky; hasLabel = true;
            string speed = sky.ScaleTag == 0 ? "Paused" : sky.ScaleTag.ToString(CultureInfo.InvariantCulture) + "×";
            string utc = FormattableString.Invariant($"UTC {sky.UtcYear:D4}-{sky.UtcMonth:D2}-{sky.UtcDay:D2} {sky.UtcHour:D2}:{sky.UtcMinute:D2}:{sky.UtcSecond:D2}");
            string local = sky.Supported
                ? FormattableString.Invariant($"NY {sky.LocalYear:D4}-{sky.LocalMonth:D2}-{sky.LocalDay:D2} {sky.LocalHour:D2}:{sky.LocalMinute:D2}:{sky.LocalSecond:D2} UTC-{Math.Abs(sky.UtcOffsetSeconds) / 3600:D2}:00")
                : "NY / sun unavailable for this date";
            Text = utc + "\n" + local + "   " + speed;
        }
    }
}
