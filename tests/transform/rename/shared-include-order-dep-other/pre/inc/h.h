#pragma once
namespace a { int f(int); }
namespace a { inline int call() { return f(1); } }
