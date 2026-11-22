#include "side.h"
// §5.6 NamespaceData `flags-inline`: an inline namespace keeps its key; the bit is in its stream.
#ifdef B
inline namespace N { int x; }
#else
namespace N { int x; }
#endif
