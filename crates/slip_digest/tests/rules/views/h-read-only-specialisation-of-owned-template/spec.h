#ifdef B
namespace acme { template <> struct S<int> {}; }
#else
namespace legacy { template <> struct S<int> {}; }
#endif
