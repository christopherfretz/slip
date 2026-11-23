#include "dep.h"

namespace legacy {
class Mine {
  int secret = 1;
  template <class T> friend auto peek(const T &t) -> decltype(t.secret);
};
}

int user() { return legacy::peek(legacy::Mine()); }
