#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 (S7): a nested operand is its own entry, after the one that
// holds it.
decltype(sizeof(decltype(make(x)))) n = 0;
