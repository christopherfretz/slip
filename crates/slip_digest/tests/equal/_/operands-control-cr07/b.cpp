// WP-156 fix round 3 (negative control, the third review's cr07):
// B1: `__underlying_type(decltype(e))` and an explicit instantiation elsewhere.
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace acme {
enum class E : unsigned { a };
unsigned g(E e) { return static_cast<__underlying_type(decltype(e))>(e); }
template <class T> T idf(T t) { return t; }
template unsigned idf<unsigned>(unsigned);
}
