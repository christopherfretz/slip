#include "side.h"
// §5.6 NamespaceData `flags-inline`: `inline` on the first declaration only, or on every one.
inline namespace N { void a(); }
#ifdef B
inline namespace N { void b(); }
#else
namespace N { void b(); }
#endif
