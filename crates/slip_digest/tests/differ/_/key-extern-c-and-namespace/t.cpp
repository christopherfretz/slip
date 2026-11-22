#include "side.h"
// digest §3.4, WP-039 item 1: the same name in an extern "C" block and outside: two keys.
extern "C" { int f(); }
#ifdef B
namespace n { int f(); }
#endif
