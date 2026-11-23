#include "side.h"
// digest §3.3, WP-039 item 1: a member pointer declarator.
#ifdef B
struct K {}; typedef struct { int m; } K::*MP2, *T;
#else
struct K {}; typedef struct { int m; } K::*MP, *T;
#endif
