#include "box.h"
namespace acme {
template <class T> struct box<T *> { T *p; };
}
