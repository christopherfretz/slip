#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `#pragma pack` where a class template is instantiated does nothing: the pattern's applies (kiln_sema `templ/class.rs:2557-2561`).
template <class T> struct X { char c; T t; };
#ifdef B
#pragma pack(push, 1)
X<int> x;
#pragma pack(pop)

#else
X<int> x;
#endif
