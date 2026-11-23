#include "side.h"
// §5.3 DeclKind `UnresolvedUsingIfExists`: added: a using-declaration whose target does not exist.
namespace n {}
#ifdef B
using n::g __attribute__((using_if_exists));
#endif
