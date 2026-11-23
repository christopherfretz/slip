// std: c++20
#include "side.h"
// WP-047: an instantiation both sides have that changes (the prospective
// destructor selected; differ/FnSpec/0-not-selected). The constraint names
// `c<T>`, so the destructors keep their keys (WP-036: a trailing
// requires-clause is in the key, §3.3) and `c` changes.
#ifdef B
template <class T> constexpr bool c = sizeof(T) == 8;
#else
template <class T> constexpr bool c = sizeof(T) == 4;
#endif
template <class T> struct X { ~X() requires c<T>; ~X(); };
X<int> x;
