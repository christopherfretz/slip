#pragma once
#include "dep.h"
namespace legacy {
struct Mine { int value; };

template <class T>
enable_if_t<sizeof(T) == 100, int> pick(T);
int pick(...);

template <class T>
auto measure(T t) -> decltype(t.size());
int measure(...);

int use();
}
