#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `#pragma pack` around a class template's definition reaches its instantiation (Kiln copies the pattern's, kiln_sema `templ/class.rs:2561`).
#ifdef B
#pragma pack(push, 1)
template <class T> struct X { char c; T t; };
#pragma pack(pop)

#else
template <class T> struct X { char c; T t; };
#endif
X<int> x;
