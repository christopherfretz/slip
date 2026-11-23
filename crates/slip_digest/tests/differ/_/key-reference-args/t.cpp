#include "side.h"
// digest §3.4, WP-039 item 1: specialisations on references to two variables: two keys.
int g1, g2;
template <int &> struct R {};
template <> struct R<g1> {};
#ifdef B
template <> struct R<g2> {};
#endif
