#include "side.h"
// digest §3.4, WP-039 item 1: two friend declarations: two keys.
struct F1; struct F2;
struct S { friend struct F1;
#ifdef B
friend struct F2;
#endif
};
