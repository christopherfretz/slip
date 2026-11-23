#include "side.h"
// §7 "Parentheses", F2: in a requires-expression outside a profile, `&T::m` against `&(T::m)`.
template<class T> concept C = requires (T t) {
#ifdef B
  &(T::m);
#else
  &T::m;
#endif
};
