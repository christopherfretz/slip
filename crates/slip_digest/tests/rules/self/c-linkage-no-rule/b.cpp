namespace legacy { extern "C" int cf(int); }
int use() { return legacy::cf(1); }
