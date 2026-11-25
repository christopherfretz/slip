#pragma once
#define WRAP(x) [] { return x; }()
#define OUTER(x) WRAP(x)
