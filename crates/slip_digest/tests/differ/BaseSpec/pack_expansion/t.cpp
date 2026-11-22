#include "side.h"
// §5.5 BaseSpec `pack_expansion`.
#ifdef B
template <class... T> struct D {}; template <class... T> struct C : D<T>... {};
#else
template <class... T> struct D {}; template <class... T> struct C : D<T...> {};
#endif
