#include "side.h"
// §5.3 DeclKind `UsingDirective`: added.
namespace n {}
#ifdef B
using namespace n;
#endif
