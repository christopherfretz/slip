#include "side.h"
// WP-156 fix round 2 (X7): a friend function template specialisation
// whose declarator holds an operand. Kiln links its declarator's types to
// nothing (its function is the specialisation): a friend specialisation
// whose function type is that of such a declarator is indeterminate
// (`kiln/friend-specialization-declarator`).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
template <class T> void tf(T);
struct F2 { friend void tf<>(Wr<decltype(make(x))>); };
