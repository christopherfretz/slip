// WP-156 fix round 3 (negative control; fix round 2's STOP (b), now a
// read): the member specialisation `Q3<int, int>::s`, written with no
// operand, of a class an instantiated body names through a template-id
// with an operand (`Q3<T, decltype(make(x))>`, T = int). Its qualifier's
// entries are written (L4's T2) and equal on both sides.
namespace acme {
namespace lib { struct X {}; int make(X); }
lib::X x;
template <class T, class U> struct Q3 { static int s; };
template <class T, class U> int Q3<T, U>::s = 0;
template <class T> int f() { return sizeof(Q3<T, decltype(make(x))>); }
int z = f<int>();
template <> int Q3<int, int>::s = 1;
}
