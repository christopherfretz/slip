#include "side.h"
// §5.3 DeclNode `UsingDirective.data`: a content-keyed unit.
namespace a {} namespace b {}
#ifdef B
using namespace b;
#else
using namespace a;
#endif
