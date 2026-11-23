#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `packed` on a friend redeclaration in a class template's pattern applies to the class.
struct S;
#ifdef B
template <class T> struct Y { friend struct __attribute__((packed)) S; };
#else
template <class T> struct Y { friend struct S; };
#endif
struct S { char c; int i; };
