#include "side.h"
// §5.6 NamespaceData, digest §5: a namespace reopened is one unit; its members are units of their own.
#ifdef B
namespace N { void a(); int x = 1; }
namespace N { void b(); int y = 2; }
#else
namespace N { void a(); int x = 1; void b(); int y = 2; }
#endif
