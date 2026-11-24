#pragma once
namespace d { int f(int); }
namespace a { inline int call() { return d::f(1); } }
