#include "side.h"
// §10.1 AttrNode: `PREFERRED_NAME.b`: the fact differs.
template <class T> struct X;
using I = X<int>;
using L = X<long>;
#ifdef B
template <class T> struct [[clang::preferred_name(L)]] X {};
#else
template <class T> struct [[clang::preferred_name(I)]] X {};
#endif
