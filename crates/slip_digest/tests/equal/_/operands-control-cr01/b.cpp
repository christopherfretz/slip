// WP-156 fix round 3 (negative control, the third review's cr01):
// B1: a type written in an expression (`static_cast<decltype(k)>(1)`) and an explicit instantiation elsewhere.
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace acme {
int k = 0;
int f() { return static_cast<decltype(k)>(1); }
template <class T> void use(T) {}
extern template void use<int>(int);
}
