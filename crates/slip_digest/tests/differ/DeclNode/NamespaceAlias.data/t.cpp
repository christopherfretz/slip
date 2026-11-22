#include "side.h"
// §5.3 DeclNode `NamespaceAlias.data`.
namespace a {} namespace b {}
#ifdef B
namespace x = b;
#else
namespace x = a;
#endif
