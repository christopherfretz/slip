// WP-156 fix round 3 (negative control, the third review's cr05):
// B2: a member's explicit instantiation whose class has a literal argument (`template void QI<3>::f();`, T1).
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace acme {
template <int N> struct QI { void f() {} };
template void QI<3>::f();
}
