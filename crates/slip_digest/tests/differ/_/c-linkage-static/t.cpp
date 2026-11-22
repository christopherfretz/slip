#include "side.h"
// digest-kinds §5.2, [dcl.link]/1: a `static` name has no language
// linkage, so `extern "C"` does not make it `c_linkage`.
namespace a { extern "C" { static int y; } }
#ifdef B
namespace b { extern "C" { static int y; } }
#endif
