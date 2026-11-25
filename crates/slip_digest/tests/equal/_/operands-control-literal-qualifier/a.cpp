// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// an explicit specialisation with a literal non-type argument, out-of-line member
namespace legacy {
template <int N> struct Q { void f(); static int s; };
template <> struct Q<1> { void f(); static int s; };
void Q<1>::f() {}
int Q<1>::s = 2;
template <int N> int Q<N>::s = 0;
template <> int Q<2>::s = 3;
}
