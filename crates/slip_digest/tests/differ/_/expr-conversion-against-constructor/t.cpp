#include "side.h"
// digest §6 _ `expr-conversion-against-constructor`.
struct From;
#ifdef B
struct To { To(const From&); };
struct From { };
From fr;
To v = fr;
#else
struct To { };
struct From { operator To() const; };
From fr;
To v = fr;
#endif
