#include "side.h"
// digest §3.4, WP-039 item 1: unrelated control: two keys.
struct T {};
#ifdef B
typedef int T2; struct T2b {};
#endif
