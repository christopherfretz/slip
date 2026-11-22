// std: c++20
#include "side.h"
// WP-047: an instantiation both sides have that changes (the prospective destructor selected; differ/FnSpec/0-not-selected).
// The class is completed and its destructor not used: no destructor's
// exception specification is resolved (digest-kinds §11.2).
#ifdef B
template <class T> struct X { ~X() requires (sizeof(T) == 8); ~X(); };
static_assert(sizeof(X<int>) == 1);
#else
template <class T> struct X { ~X() requires (sizeof(T) == 4); ~X(); };
static_assert(sizeof(X<int>) == 1);
#endif
