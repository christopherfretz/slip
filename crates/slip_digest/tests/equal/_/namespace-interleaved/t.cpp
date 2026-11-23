#include "side.h"
// §5.6 NamespaceData, digest §5: the members of two namespaces interleaved differently.
#ifdef B
namespace N { void a(); }
namespace M { void c(); }
namespace N { void b(); }
#else
namespace N { void a(); void b(); }
namespace M { void c(); }
#endif
