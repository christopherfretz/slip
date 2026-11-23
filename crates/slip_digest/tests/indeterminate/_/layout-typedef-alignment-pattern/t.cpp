#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a class template's field typed by the typedef: the pattern and the instantiation's `definition` (whose fields are derived, so the walk reads Kiln's fields, not the member list).
typedef int I __attribute__((aligned(8)));
template <class T> struct X { char c; I i; T t; };
X<char> x;
