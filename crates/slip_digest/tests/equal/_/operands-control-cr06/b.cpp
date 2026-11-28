// WP-156 fix round 3 (negative control, the third review's cr06):
// B1: `sizeof(V2<decltype(k)>)` and an explicit specialisation `template <> void V2<int>::f()` (T2).
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace acme {
template <class T> struct V2 { void f(); };
template <class T> void V2<T>::f() {}
int k = 0;
unsigned long g() { return sizeof(V2<decltype(k)>); }
template <> void V2<int>::f() {}
}
