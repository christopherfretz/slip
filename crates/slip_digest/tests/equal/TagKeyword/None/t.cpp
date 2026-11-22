// std: c++20
#include "side.h"
// §4.3 TagKeyword `None`: canonical as `Typename` (kiln_ast/src/types/canon.rs:899 `canonical_keyword`), so no canonical type holds it.
template <class T> struct A {
#ifdef B
using X = T::x;
#else
using X = typename T::x;
#endif
};
