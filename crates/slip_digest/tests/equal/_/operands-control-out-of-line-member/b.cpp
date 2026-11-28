// WP-156 fix round 3 (negative control of T2's amended range, the
// coordinator's ruling on fact (d)): an out-of-line member of a class
// template, `QP<T>::f`, whose qualifier is a template-id. T2 writes the
// qualifier's entries only; the body's `make(x)` and the parameter's `X1`
// are read by the stream, not by site 9. A correct relocation: equal.
namespace acme {
namespace lib { struct X {}; int make(X); }
lib::X x;
struct X1 { int a; };
template <class T> struct QP { void f(X1 a); };
template <class T> void QP<T>::f(X1 a) { make(x); }
template struct QP<int>;
}
