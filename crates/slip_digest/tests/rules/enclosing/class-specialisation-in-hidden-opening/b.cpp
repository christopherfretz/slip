namespace n { template <class T> struct S { int f() { return 0; } }; }
namespace n { template <> struct S<int> { int f() { return 1; } virtual ~S(); }; }
n::S<int>::~S() {}
int u() { return n::S<int>().f(); }
