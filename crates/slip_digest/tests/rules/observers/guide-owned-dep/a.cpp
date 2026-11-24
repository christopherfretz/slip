#include "d.h"
namespace old { Box(const char *) -> Box<const char *>; int m; }
int u() { return old::m; }
