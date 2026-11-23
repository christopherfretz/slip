#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a class template's field of the typedef. `X<char>::<definition>` is equal: its layout follows from the pattern's field `X::i`, as a class's layout from its fields.
typedef int I __attribute__((aligned(8)));
#ifdef B
template <class T> struct X { char c; I i; T t; };
#else
template <class T> struct X { char c; int i; T t; };
#endif
X<char> x;
