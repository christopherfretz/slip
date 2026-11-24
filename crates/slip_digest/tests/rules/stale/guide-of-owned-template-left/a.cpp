namespace legacy { template <class T> struct W { W(T); }; W(int) -> W<long>; }
legacy::W<int> u() { return legacy::W<int>(1); }
