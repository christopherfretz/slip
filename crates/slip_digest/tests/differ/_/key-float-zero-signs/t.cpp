#include "side.h"
// digest §3.4, WP-039 item 1: specialisations on 0.0 against -0.0: two keys.
template <auto> struct N {};
template <> struct N<0.0> {};
#ifdef B
template <> struct N<-0.0> {};
#endif
