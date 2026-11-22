#include "side.h"
// §10.1 AttrKind: `PREFERRED_NAME`: the attribute on one side only.
template <class T> struct X;
using I = X<int>;
#ifdef B
template <class T> struct [[clang::preferred_name(I)]] X {};
#else
template <class T> struct X {};
#endif
