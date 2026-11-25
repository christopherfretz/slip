#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 item 3 (S3): `v` is declared with a placeholder: its deduced type
// is not read, so its unit has no operand unit. `w`'s operand moves.
decltype(make(x)) w = 0;
auto v = w;
decltype(auto) v2 = w;
