#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 fix round 1 (W1, W2, W6, W8): the holders of a written type in a
// declaration that the first delivery did not read, each with an operand
// whose binding moves (`lib::make` against `::make<lib::X>`).
template <int N> struct W2 { int f() { return N; } };
template <class T> struct Wr { T m; };
// W1: a non-type template parameter's type; in a template template
// parameter's head; in a member template of an instantiated class.
template <decltype(make(x)) N> struct P1 {};
template <template <decltype(make(x)) N> class TT> struct P2 {};
template <class T> struct I1 { template <decltype(make(x)) N> int g() { return N; } };
I1<int> i1;
// W2: an enumeration's fixed underlying type, on a definition and on an
// opaque declaration.
enum E1 : decltype(make(x)) { e1 };
enum class E2 : decltype(make(x));
// W6: a using-declaration's qualifier (`W2<4>` is the same class).
struct U1 : W2<4> { using W2<sizeof(make(x))>::f; };
// A variable template specialisation's written type.
template <class T> int vt = 0;
template <> decltype(make(x)) vt<int> = 1;
// W8: a class's explicit instantiation keeps its arguments as written.
template struct W2<sizeof(make(x)) + 1>;
// W1 with a constrained placeholder (mutation R18): the parameter's type
// is the placeholder as written, its type-constraint read.
template <class T, class U> concept Same = __is_same(T, U);
template <Same<decltype(make(x))> auto N> struct P3 {};
