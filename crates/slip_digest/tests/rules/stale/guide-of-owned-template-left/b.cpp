namespace acme { template <class T> struct W { W(T) {} }; }
namespace legacy { using acme::W; W(int) -> W<long>; }
acme::W<int> u() { return acme::W<int>(1); }
