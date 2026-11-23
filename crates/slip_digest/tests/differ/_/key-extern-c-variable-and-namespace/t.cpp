#include "side.h"
// digest §3.4, WP-039 item 1: the same variable name in an extern "C" block and outside: two keys.
extern "C" { extern int v; }
#ifdef B
namespace n { extern int v; }
#endif
