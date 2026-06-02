# syntax=docker/dockerfile:1
FROM node:20-alpine
WORKDIR /app
COPY package.json package-lock.json ./
# Use `npm install` rather than `npm ci`: Baileys' optional `sharp` dependency
# pulls platform-specific @img/* packages that npm does not reliably record in
# package-lock.json, which makes the strict `npm ci` lock check fail. `npm install`
# reconciles instead of hard-failing.
RUN npm install --omit=dev --no-audit --no-fund
COPY . .

# Baileys persists pairing credentials under ./auth_state -> mount a volume here.
ENV BACKEND_URL=http://backend:8080
CMD ["node", "index.js"]
