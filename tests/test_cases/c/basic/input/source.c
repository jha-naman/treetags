// sourced from https://learnxinyminutes.com/c

#define DAYS_IN_YEAR 365

enum days {SUN = 1, MON, TUE, WED = 99, THU, FRI, SAT};

#include <stdlib.h>
#include "../my_lib/my_lib_header.h"

void function_1();
int function_2(void);

int add_two_ints(int x1, int x2);
int main (int argc, char** argv)
{
  printf("%d\n", 0);
  int input;
  scanf("%d", &input);

  int x_int = 0;

  unsigned short ux_short;
  unsigned long long ux_long_long;

  char my_char_array[20];
  int my_array[5] = {1, 2};
  printf("%d\n", my_array[1]);

  char a_string[20] = "This is a string";
  printf("%s\n", a_string);

 
  int multi_array[2][5] = {
    {1, 2, 3, 4, 5},
    {6, 7, 8, 9, 0}
  };

  int i1 = 1, i2 = 2;
  float f1 = 1.0, f2 = 2.0;

  int b, c;
  b = c = 0;

  int *px, not_a_pointer;
  px = &x;
  printf("%p\n", (void *)px);
  printf("%zu, %zu\n", sizeof(px), sizeof(not_a_pointer));

  function_1();
}

int add_two_ints(int x1, int x2)
{
  return x1 + x2;
}

void swapTwoNumbers(int *a, int *b)
{
    int temp = *a;
    *a = *b;
    *b = temp;
}

int i = 0;
void testFunc() {
  extern int i;
}

static int j = 0;
void testFunc2() {
  extern int j;
}
typedef int my_type;
my_type my_type_var = 0;

struct rectangle {
  int width;
  int height;
};

struct {
  int x;
  int y;
} anonymous_struct_var;

void function_1()
{
  struct rectangle my_rec = { 1, 2 };

 
  my_rec.width = 10;
  my_rec.height = 20;

 
  struct rectangle *my_rec_ptr = &my_rec;

 
  (*my_rec_ptr).width = 30;

 
  my_rec_ptr->height = 10;
}

typedef struct rectangle rect;

int area(rect r)
{
  return r.width * r.height;
}

typedef struct {
  int width;
  int height;
} rect;

rect r;

struct rectangle r;

int areaptr(const rect *r)
{
  return r->width * r->height;
}

void str_reverse_through_pointer(char *str_in) {
  void (*f)(char *);
  f = &str_reverse;
  (*f)(str_in);
}

typedef void (*my_fnp_type)(char *);

#ifndef EXAMPLE_H
#define EXAMPLE_H

#include <string.h>

#define EXAMPLE_NAME "Dennis Ritchie"

#define ADD(a, b) ((a) + (b))
typedef struct Node
{
    int val;
    struct Node *next;
} Node;

enum traffic_light_state {GREEN, YELLOW, RED};

Node createLinkedList(int *vals, int len);

#endif

