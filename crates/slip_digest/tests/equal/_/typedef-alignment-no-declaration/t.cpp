#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): no declaration carries an alignment: transparent and determinate.
typedef int R;
typedef int R;
#ifdef B
R v;
#else
int v;
#endif
