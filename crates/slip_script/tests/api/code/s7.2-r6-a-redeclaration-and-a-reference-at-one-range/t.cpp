namespace o { int x(int); }
#define M(y) namespace o { int y(int); } int k = o::y(1);
M(x)
