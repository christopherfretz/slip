#include "side.h"
// `extern` on a namespace-scope `const` variable gives it external
// linkage ([basic.link]/3.2): side B's `x` is a different entity in
// linking from side A's internal one, so the unit must differ.
#ifdef B
extern const int x = 1;
#else
const int x = 1;
#endif
