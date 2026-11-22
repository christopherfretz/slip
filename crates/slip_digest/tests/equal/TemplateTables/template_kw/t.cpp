#include "side.h"
// §9.5 TemplateTables `template_kw`: the `template` keyword after a qualifier
// is read by the printer only.
namespace N { template <class T> int v = 1; }
#ifdef B
int x = N::template v<int>;
#else
int x = N::v<int>;
#endif
