#pragma once
namespace a { int g(int); }
namespace a { inline int call() { return g(1); } }
