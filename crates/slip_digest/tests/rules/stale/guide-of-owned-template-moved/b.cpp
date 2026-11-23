namespace acme { template <class T> struct W { W(T) {} }; W(int) -> W<long>; }
acme::W<int> u() { return acme::W<int>(1); }
