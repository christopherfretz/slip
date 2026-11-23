namespace N {
int v;
}
int a = N::v;
#include "h.h"
int b = N::v + N::v;
