#include "side.h"
// §5.3 DeclKind `NamespaceAlias`: added.
namespace n {}
#ifdef B
namespace m = n;
#endif
