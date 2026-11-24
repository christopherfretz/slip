#include "d.h"
namespace old { Box(const char *) -> Box<const char *>; }
namespace fresh { int m; }
int u() { return fresh::m; }
