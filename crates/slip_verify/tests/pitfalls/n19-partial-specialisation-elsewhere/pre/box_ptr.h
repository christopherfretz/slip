#include "box.h"
namespace legacy {
template <class T> struct box<T *> { T *p; };
}
