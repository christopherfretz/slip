int g(int);
#define W(x) (x)
#define NAME a
#include "h.h"
#undef W
#undef NAME
int W(int);
#define NAME b
#include "h.h"
