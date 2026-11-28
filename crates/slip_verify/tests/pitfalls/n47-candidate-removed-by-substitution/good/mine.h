#pragma once
#include "dep.h"
namespace acme {
struct Mine { int value; };

template <class T>
legacy::enable_if_t<sizeof(T) == 100, int> pick(T);
int pick(...);

template <class T>
auto measure(T t) -> decltype(t.size());
int measure(...);

int use();
}
