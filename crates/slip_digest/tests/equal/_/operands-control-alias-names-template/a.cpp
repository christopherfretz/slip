// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// an alias template naming a class template: an explicit specialisation's member
namespace legacy {
template <class T> struct V { void f(); };
template <class T> using Vec = V<T>;
template <> void V<int>::f() {}
}
