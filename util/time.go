package util

import (
	"fmt"
	"time"
)

// ThreadSleep is a helper function to sleep for a given duration.
func ThreadSleep(d time.Duration) {
	time.Sleep(d)
}

// Time is time data type.
type Time time.Duration

func (t *Time) setTimeFromString(str string) {
	var h, m, s, n int
	_, err := fmt.Sscanf(str, "%02d:%02d:%02d.%09d", &h, &m, &s, &n)
	if err != nil {
		return
	}
	*t = newTime(h, m, s, n)
}

// NewTime is a constructor for Time and returns new Time.
func NewTime(hour, min, sec, nsec int) Time {
	return newTime(hour, min, sec, nsec)
}

func newTime(hour, min, sec, nsec int) Time {
	return Time(
		time.Duration(hour)*time.Hour +
			time.Duration(min)*time.Minute +
			time.Duration(sec)*time.Second +
			time.Duration(nsec)*time.Nanosecond,
	)
}

func (t Time) hours() int {
	return int(time.Duration(t).Truncate(time.Hour).Hours())
}

func (t Time) minutes() int {
	return int((time.Duration(t) % time.Hour).Truncate(time.Minute).Minutes())
}

func (t Time) seconds() int {
	return int((time.Duration(t) % time.Minute).Truncate(time.Second).Seconds())
}

func (t Time) nanoseconds() int {
	return int((time.Duration(t) % time.Second).Nanoseconds())
}
