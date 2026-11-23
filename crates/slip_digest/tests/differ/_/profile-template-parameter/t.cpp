#include "side.h"
// §4.4, §7 _ `profile-template-parameter`: a profile naming a template parameter.
#ifdef B
template<int N> void f(char(*)[N + 1]);
template<int N> void f(char(*)[N + 2]);
#else
template<int N> void f(char(*)[N + 1]);
#endif
