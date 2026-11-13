-- A small Lua program that exercises most of the syntax.
--[==[ a long comment
with ]] inside ]==]

local Stack = {}
Stack.__index = Stack

function Stack.new(...)
  local self = setmetatable({items = {...}, size = select("#", ...)}, Stack)
  return self
end

function Stack:push(value)
  self.size = self.size + 1
  self.items[self.size] = value
end

local function sum(list)
  local total <const> = 0
  for i = 1, #list, 1 do total = total + list[i] end
  return total
end

local text = [[
long string, no escapes: \n stays]]
local escaped = "tab\tquote\" zap \z
                 continued"
local codes = "\x41\65\u{20AC}\\\
next line"
local s = Stack.new(1, 2, 3)
s:push(2 ^ -1)
print(sum{1, 2; 3}, text .. escaped, 0x1F, 0xA.8p1, 3e-2, .5)
for k, v in pairs({a = 1, [2] = "b", 'c'}) do
  if k == "a" and not v ~= 1 then goto continue
  elseif type(k) == "number" then print(k // 2, k % 2, -k ^ 2)
  else print(k) end
  ::continue::
end
repeat local x = 1 until x >= 1 or 1 << 2 | 3 & ~4 ~ 5 > 2
while false do break end
do ; end
