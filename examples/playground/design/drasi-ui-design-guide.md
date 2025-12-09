# Drasi Sandbox UI Design Suggestions

## Design Philosophy

The design follows these core principles:
- **Clean & Modern**: Minimal visual clutter with clear hierarchies
- **Professional**: Suitable for technical users while remaining approachable
- **Functional**: Every element serves a purpose with clear affordances
- **Responsive**: Adapts to different screen sizes and workflows

## Color Palette

### Primary Colors
- **Blue-600** (#2563EB): Primary actions, active states
- **Purple-600** (#9333EA): Accent color for gradients
- **Green-600** (#059669): Success states, execute actions
- **Gray-900** (#111827): Primary text

### Neutral Colors
- **Gray-50** (#F9FAFB): Background
- **Gray-100** (#F3F4F6): Subtle backgrounds
- **Gray-200** (#E5E7EB): Borders
- **Gray-500** (#6B7280): Secondary text
- **Gray-700** (#374151): Icon colors

### Semantic Colors
- **Yellow-50** (#FEFCE8): Highlight changed rows
- **Red-600** (#DC2626): Destructive actions
- **Green-100/500**: Active/live status indicators

## Typography

- **Headers**: Inter or system font, semi-bold
- **Body**: Inter or system font, regular
- **Code/Query**: JetBrains Mono or Fira Code
- **Sizes**: 
  - Headers: 24px (h1), 18px (h2), 16px (h3)
  - Body: 14px
  - Small text: 12px
  - Code: 13px

## Layout Structure

### 1. Split-Panel Design
- **50/50 split**: Equal emphasis on input (data/query) and output (results)
- **Resizable divider**: Allow users to adjust panel sizes
- **Collapsible panels**: Option to focus on one area

### 2. Header Bar
- **Logo/Branding**: Gradient icon with product name
- **Global Actions**: Documentation, Share, Settings
- **User Profile**: Optional authentication indicator

### 3. Left Panel (Input)
- **Tab Navigation**: Switch between Source Data and Query Editor
- **Contextual Toolbar**: Actions relevant to active tab
- **Content Area**: Table editor or code editor

### 4. Right Panel (Output)
- **Status Bar**: Query status, update indicators
- **Results Table**: Clean data presentation
- **Change Log**: Visual timeline of data changes

## Interactive Elements

### 1. Data Table Editor
- **Inline Editing**: Click to edit cells directly
- **Row Actions**: Hover to reveal edit/delete buttons
- **Bulk Operations**: Select multiple rows for batch actions
- **Data Types**: Visual indicators for different data types
- **Validation**: Real-time validation with error states

### 2. Query Editor
- **Syntax Highlighting**: Language-specific highlighting
- **Auto-complete**: Suggestions based on schema
- **Error Indicators**: Inline error highlighting
- **Query History**: Dropdown with recent queries
- **Templates**: Quick-start query examples

### 3. Results Display
- **Live Updates**: Animated indicator for active queries
- **Diff Highlighting**: Show changed rows/cells
- **Pagination**: For large result sets
- **Export Options**: CSV, JSON, clipboard copy
- **Visualization**: Optional chart view for results

## Micro-interactions

1. **Hover States**: Subtle background color changes
2. **Focus States**: Clear blue outline for accessibility
3. **Loading States**: Skeleton screens, not spinners
4. **Transitions**: 150ms ease-out for all animations
5. **Success Feedback**: Brief green flash on successful operations

## Alternative Design Variations

### Variation 1: Dark Mode
```css
--bg-primary: #0F172A
--bg-secondary: #1E293B
--text-primary: #F1F5F9
--border: #334155
```

### Variation 2: Vertical Layout
- Query editor on top (30% height)
- Source data and results side-by-side below
- Better for wide screens

### Variation 3: Monaco Editor Integration
- Use Monaco Editor for query editing
- Provides VS Code-like experience
- Better for complex queries

### Variation 4: Floating Panels
- Draggable, resizable panels
- Dock/undock capability
- Save layout preferences

## Additional Features to Consider

### 1. Query Builder UI
- Visual query builder for non-technical users
- Drag-and-drop nodes
- Automatic Cypher/GQL generation

### 2. Schema Visualization
- Graph view of data relationships
- Interactive node exploration
- Right-click context menus

### 3. Performance Metrics
- Query execution time
- Update frequency graph
- Resource usage indicators

### 4. Collaboration Features
- Share sandbox via URL
- Real-time collaboration
- Comment on queries

### 5. Mobile Responsive Design
- Stack panels vertically
- Touch-friendly controls
- Swipe gestures for panel navigation

## Accessibility Considerations

1. **Color Contrast**: All text meets WCAG AA standards
2. **Keyboard Navigation**: Full keyboard support
3. **Screen Readers**: Proper ARIA labels
4. **Focus Management**: Clear focus indicators
5. **Error Messages**: Descriptive, actionable errors

## Technical Implementation

### Recommended Stack
- **Framework**: React with TypeScript
- **Styling**: Tailwind CSS or Emotion
- **State Management**: Zustand or Redux Toolkit
- **Data Grid**: AG-Grid or React Table
- **Code Editor**: Monaco Editor or CodeMirror
- **Icons**: Lucide React or Heroicons

### Performance Optimizations
- Virtual scrolling for large datasets
- Debounced live updates
- Web Workers for query parsing
- IndexedDB for browser storage
- WebSocket for real-time updates

## Design System Components

### Buttons
- Primary: Blue background, white text
- Secondary: White background, gray border
- Danger: Red background for destructive actions
- Icon buttons: 32px square, subtle hover

### Forms
- Input fields: 40px height, gray border
- Labels: 12px uppercase, gray-500
- Validation: Red border, error message below
- Dropdowns: Custom styled, not native

### Tables
- Sticky headers
- Zebra striping optional
- Hover row highlighting
- Sortable columns

### Modals
- Centered overlay
- Smooth fade-in animation
- Clear close button
- Focus trap

## Conclusion

This design system provides a solid foundation for a professional Drasi sandbox UI. The clean, modern aesthetic combined with powerful functionality will create an excellent user experience for developers experimenting with continuous queries. The modular approach allows for easy customization and extension as the product evolves.