#include "side.h"
// §5.6 NamespaceData `flags-anonymous` (keyed): the name of an unnamed namespace is in the key.
#ifdef B
namespace a { int x; }
#else
namespace { int x; }
#endif
