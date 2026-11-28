// WP-156 fix round 3 (negative control, the third review's cr02):
// B1: `Q3<int, decltype(k)>::s` in a function and the member specialisation `Q3<int, int>::s`.
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace acme {
template <class T, class U> struct Q3 { static int s; };
template <class T, class U> int Q3<T, U>::s = 0;
template <> int Q3<int, int>::s = 1;
int k = 0;
int f() { return Q3<int, decltype(k)>::s; }
}
