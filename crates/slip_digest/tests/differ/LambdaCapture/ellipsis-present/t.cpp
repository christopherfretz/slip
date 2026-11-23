#include "side.h"
// §10.4 LambdaCapture `ellipsis-present`: a pack capture.
#ifdef B
template <class... T> int f(T... t) { return [t...] { return (int)sizeof...(T); }(); }
#else
template <class... T> int f(T... t) { return [] { return (int)sizeof...(T); }(); }
#endif
