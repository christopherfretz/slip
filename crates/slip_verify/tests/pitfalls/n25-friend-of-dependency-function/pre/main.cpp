#include "dep.h"

namespace legacy {
class Mine {
  int secret = 1;
  template <class T> friend int peek(const T &);
};
}

int user() { return legacy::peek(legacy::Mine()); }
