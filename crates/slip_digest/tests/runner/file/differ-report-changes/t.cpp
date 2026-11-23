// std: c++20
#include "side.h"
// WP-047: an instantiation both sides have that changes (the prospective destructor selected; differ/FnSpec/0-not-selected).
#ifdef B
template <class T> struct X { ~X() requires (sizeof(T) == 8); ~X(); };
X<int> x;
#else
template <class T> struct X { ~X() requires (sizeof(T) == 4); ~X(); };
X<int> x;
#endif
